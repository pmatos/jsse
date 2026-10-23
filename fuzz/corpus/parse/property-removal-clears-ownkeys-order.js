// A property lives in two coordinated structures: the property map (used by
// [[Get]] / [[GetOwnProperty]] / hasOwnProperty) and the ordered key list (used
// by OrdinaryOwnPropertyKeys — the source for Object.getOwnPropertyNames,
// Reflect.ownKeys and for-in). Removing a property must clear it from BOTH: an
// engine that only deletes from the map leaks the key back through enumeration.
// This exercises the [[Delete]] path (sec-ordinarydelete) that Object.getOwn-
// PropertyNames (sec-object.getownpropertynames) then observes.
// Spec: ECMAScript 2024, sec-ordinarydelete step 4 ("Remove the own property
//       named P from O"), sec-ordinaryownpropertykeys.

function Test262Error(message) {
  this.message = message || "";
}
Test262Error.prototype.toString = function () {
  return "Test262Error: " + this.message;
};

function assertKeys(actual, expected, label) {
  var a = JSON.stringify(actual);
  var e = JSON.stringify(expected);
  if (a !== e) {
    throw new Test262Error(label + ": expected " + e + ", got " + a);
  }
}

// delete removes the key from getOwnPropertyNames, not just from lookup.
var o = { a: 1, b: 2, c: 3 };
delete o.b;
if (o.hasOwnProperty("b")) {
  throw new Test262Error("delete: b still an own property");
}
assertKeys(Object.getOwnPropertyNames(o), ["a", "c"], "delete leaves stale key in ownKeys");
assertKeys(Reflect.ownKeys(o), ["a", "c"], "delete leaves stale key in Reflect.ownKeys");

// The relative order of the survivors is preserved (only "b" is spliced out).
var order = { x: 0, y: 0, z: 0, w: 0 };
delete order.y;
delete order.w;
assertKeys(Object.getOwnPropertyNames(order), ["x", "z"], "delete disturbed survivor order");

// for-in must not visit a deleted key.
var seen = [];
var f = { p: 1, q: 2 };
delete f.p;
for (var k in f) {
  seen.push(k);
}
assertKeys(seen, ["q"], "for-in visited a deleted key");

// Deleting then re-adding appends at the end (a fresh insertion), never revives
// the old slot position.
var reAdd = { first: 1, second: 2 };
delete reAdd.first;
reAdd.first = 9;
assertKeys(Object.getOwnPropertyNames(reAdd), ["second", "first"], "re-add did not append");
