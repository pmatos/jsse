/*---
description: >
  A named property access site that remembers where it last found a property
  (`o.x`, `o.x = v`, `o.x++`, `o.m()`) still reads, writes and updates the
  right property when the next receiver has the name at a different position,
  when the property was deleted and re-added, when the object grew past the
  inline property storage or had many properties removed, and when the holder
  moves along the prototype chain.
esid: sec-ordinary-object-internal-methods-and-internal-slots-get-p-receiver
flags: [noStrict]
---*/

function getX(o) { return o.x; }
function setX(o, v) { o.x = v; }
function incX(o) { return o.x++; }
function callM(o) { return o.m(); }

// Receivers that keep `x` at different positions, interleaved at one site.
(function () {
  var shapes = [
    { x: "a0" },
    { p: 1, x: "a1" },
    { p: 1, q: 2, x: "a2" },
    { p: 1, q: 2, r: 3, x: "a3" },
    { p: 1, q: 2, r: 3, s: 4, t: 5, u: 6, v: 7, w: 8, x: "a8" },
    { p: 1, q: 2, r: 3, s: 4, t: 5, u: 6, v: 7, w: 8, y: 9, z: 10, x: "a10" },
  ];
  for (var round = 0; round < 4; round++) {
    for (var i = 0; i < shapes.length; i++) {
      var expected = shapes[i].x;
      assert.sameValue(getX(shapes[i]), expected, "get " + i + " round " + round);
      setX(shapes[i], "w" + i + round);
      assert.sameValue(shapes[i].x, "w" + i + round, "set " + i + " round " + round);
      setX(shapes[i], expected);
    }
  }
  var objectWithoutX = { p: 1, q: 2 };
  assert.sameValue(getX(objectWithoutX), undefined, "absent after the hint was trained");
  setX(objectWithoutX, "added");
  assert.sameValue(Object.keys(objectWithoutX).join(), "p,q,x", "added last");
  assert.sameValue(getX(objectWithoutX), "added", "and found at its new slot");
})();

// Delete and re-add moves the property; deleting an earlier one shifts it.
(function () {
  var o = { a: 1, b: 2, x: "first" };
  for (var i = 0; i < 3; i++) assert.sameValue(getX(o), "first");
  delete o.a;
  assert.sameValue(getX(o), "first", "an earlier property was removed");
  setX(o, "second");
  assert.sameValue(o.x, "second", "set after the shift");
  delete o.x;
  assert.sameValue(getX(o), undefined, "deleted");
  setX(o, "third");
  assert.sameValue(Object.keys(o).join(), "b,x", "re-added last");
  assert.sameValue(getX(o), "third", "re-added value");
  assert.sameValue(incX(o), NaN, "a string updates to NaN");
  o.x = 5;
  assert.sameValue(incX(o), 5, "update returns the old value");
  assert.sameValue(o.x, 6, "update stored");
})();

// Many properties added and removed: spill, tombstones and compaction.
(function () {
  var o = {};
  for (var i = 0; i < 60; i++) o["k" + i] = i;
  o.x = "late";
  assert.sameValue(getX(o), "late", "past the inline storage");
  for (var j = 0; j < 55; j++) delete o["k" + j];
  assert.sameValue(getX(o), "late", "after most properties were removed");
  setX(o, "later");
  assert.sameValue(o.x, "later", "set after removal");
  for (var m = 55; m < 60; m++) delete o["k" + m];
  assert.sameValue(getX(o), "later", "only x left");
  assert.sameValue(Object.keys(o).join(), "x", "keys");
  var p = {};
  for (var n = 0; n < 40; n++) p["j" + n] = n;
  assert.sameValue(getX(p), undefined, "absent in a spilled object");
  p.x = 1;
  assert.sameValue(incX(p), 1, "spilled update");
  assert.sameValue(p.x, 2, "spilled update stored");
})();

// A hit on the receiver and a hit on its prototype alternate at one site.
(function () {
  var proto = { m: function () { return "proto"; }, pad1: 1, pad2: 2, pad3: 3 };
  var child = Object.create(proto);
  var own = Object.create(proto);
  own.m = function () { return "own"; };
  for (var i = 0; i < 4; i++) {
    assert.sameValue(callM(child), "proto", "inherited " + i);
    assert.sameValue(callM(own), "own", "own " + i);
  }
  delete own.m;
  assert.sameValue(callM(own), "proto", "own method deleted");
  child.m = function () { return "child own"; };
  assert.sameValue(callM(child), "child own", "own method added");
  Object.setPrototypeOf(child, { pad: 0, m: function () { return "other proto"; } });
  delete child.m;
  assert.sameValue(callM(child), "other proto", "prototype replaced");
  var grand = Object.create(Object.create({ q: 1, r: 2, m: function () { return "grand"; } }));
  assert.sameValue(callM(grand), "grand", "two levels up");
  assert.throws(TypeError, function () { callM(Object.create({ pad: 1 })); }, "no method anywhere");
})();

// A name shared by a data property and an accessor in different receivers.
(function () {
  var data = { pad: 0, x: "data" };
  var accessor = { get x() { return "getter"; }, set x(v) { accessor.log = v; } };
  for (var i = 0; i < 3; i++) {
    assert.sameValue(getX(data), "data");
    assert.sameValue(getX(accessor), "getter");
  }
  setX(accessor, "logged");
  assert.sameValue(accessor.log, "logged", "setter after a data hit");
  setX(data, "d2");
  assert.sameValue(data.x, "d2", "data store after an accessor");
  Object.defineProperty(data, "x", { writable: false });
  setX(data, "d3");
  assert.sameValue(data.x, "d2", "read-only after the hint was trained");
})();

// The same atom serves different holders: Array `length`, functions, strings.
(function () {
  function getLength(o) { return o.length; }
  function setLength(o, v) { o.length = v; }
  var a = [1, 2, 3];
  var f = function (p, q) {};
  var plain = { pad: 0, length: 7 };
  for (var i = 0; i < 3; i++) {
    assert.sameValue(getLength(a), 3);
    assert.sameValue(getLength(f), 2);
    assert.sameValue(getLength(plain), 7);
    assert.sameValue(getLength("abcd"), 4);
  }
  setLength(a, 1);
  assert.sameValue(a.length, 1, "Array length through ArraySetLength");
  assert.sameValue(a[1], undefined, "element removed");
  setLength(plain, 9);
  assert.sameValue(plain.length, 9, "plain object length");
  setLength(f, 5);
  assert.sameValue(f.length, 2, "function length is read-only");
})();

// Properties whose key text did not come from this site's identifier, and
// receivers that never take a remembered slot, at sites already trained.
(function () {
  var trained = { pad: 0, x: "trained" };
  for (var i = 0; i < 3; i++) {
    assert.sameValue(getX(trained), "trained");
    setX(trained, "trained");
  }
  var parsed = JSON.parse('{"p":1,"q":2,"x":"json"}');
  var computed = {};
  computed["p"] = 1;
  computed["x" + ""] = "computed";
  var defined = Object.defineProperty({ p: 1 }, ["x"].join(""), { value: "defined", writable: true, enumerable: true, configurable: true });
  [[parsed, "json"], [computed, "computed"], [defined, "defined"]].forEach(function (pair) {
    for (var k = 0; k < 3; k++) assert.sameValue(getX(pair[0]), pair[1], "get " + pair[1]);
    setX(pair[0], "w");
    assert.sameValue(pair[0].x, "w", "set " + pair[1]);
    assert.sameValue(incX(pair[0]), NaN, "update " + pair[1]);
  });

  var log = [];
  var proxy = new Proxy({ x: "target" }, {
    get: function (t, k) { log.push("get"); return t[k]; },
    set: function (t, k, v) { log.push("set"); t[k] = v; return true; },
  });
  assert.sameValue(getX(proxy), "target", "Proxy get after training");
  setX(proxy, "p");
  assert.sameValue(log.join(), "get,set", "Proxy traps ran");
  assert.sameValue(getX(new String("s")), undefined, "String wrapper without x");
  assert.sameValue(getX((function () { return arguments; })(1)), undefined, "arguments without x");
  var ta = new Uint8Array(2);
  setX(ta, 7);
  assert.sameValue(getX(ta), 7, "TypedArray ordinary name");
})();
