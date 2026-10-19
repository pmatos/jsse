/*---
description: >
  A var that direct eval declares in a function environment after closures
  captured it, and a mapped `arguments` write to a captured parameter, keep the
  stored object alive across minor collections.
info: |
  The generational collector does not re-scan old closures; it re-scans the
  environments that were written to since their last scan. Declaring a binding
  through eval, or writing a mapped `arguments` element, writes to an
  environment that old closures capture.
esid: sec-evaldeclarationinstantiation
features: [host-gc-required]
flags: [noStrict]
---*/

var sink;

function minor() {
  for (var i = 0; i < 30000; i++) sink = { i: i };
}

function promote() {
  $262.gc();
  minor();
  minor();
}

var serial = 0;
function fresh(label) {
  return { label: label, serial: ++serial, payload: [serial, serial + 1] };
}

function outer() {
  var api = {
    store: function (v) { added = v; },
    read: function () { return added; },
  };
  // The closures exist before eval adds the binding they resolve.
  eval("var added = null");
  return api;
}

var s = outer();
promote();

for (var round = 0; round < 3; round++) {
  var value = fresh("eval#" + round);
  s.store(value);
  minor();
  assert.sameValue(s.read(), value, "var declared by eval, round " + round);
  minor();
  minor();
  assert.sameValue(s.read(), value, "var declared by eval after collections, round " + round);
  assert.sameValue(s.read().payload[1], value.serial + 1, "payload, round " + round);
}

function mapped(a) {
  var viaArguments = arguments;
  return {
    store: function (v) { viaArguments[0] = v; },
    read: function () { return a; },
  };
}

var m = mapped(fresh("initial"));
promote();
for (var round = 0; round < 3; round++) {
  var value = fresh("mapped#" + round);
  m.store(value);
  minor();
  assert.sameValue(m.read(), value, "mapped arguments write, round " + round);
  minor();
  minor();
  assert.sameValue(m.read(), value, "mapped arguments write after collections, round " + round);
  assert.sameValue(m.read().payload[1], value.serial + 1, "payload, round " + round);
}
