/*---
description: >
  A young object stored into an environment that old closures capture stays
  alive and keeps its identity across minor collections, however the store was
  made: assignment, destructuring, loop targets, TDZ initialization, generator and async resumption, `super()`,
  per-iteration bindings and promise reactions.
info: |
  The generational collector does not re-scan old closures. It remembers an
  environment that was written to after a scan found no young reference in it,
  and re-scans exactly those at the next minor collection. A store that
  skipped that bookkeeping would let the collector free an object the
  environment still references: later reads would see a wrong value, a
  recycled object, or throw. Only the observable result is asserted: every
  read must return the object last stored.
esid: sec-environment-records
features: [host-gc-required]
flags: [async]
---*/

var sink;

// Allocates enough short-lived objects to run several minor collections.
function minor() {
  for (var i = 0; i < 30000; i++) sink = { i: i, next: sink && null };
}

// Promotes everything allocated so far.
function promote() {
  $262.gc();
  minor();
  minor();
}

var serial = 0;
function fresh(label) {
  return { label: label, serial: ++serial, payload: [serial, serial + 1] };
}

function check(label, read, expected) {
  var got = read();
  assert.sameValue(got, expected, label + ": identity");
  assert.sameValue(got.label, expected.label, label + ": label");
  assert.sameValue(got.payload[1], got.serial + 1, label + ": payload");
}

function scenario(label, make) {
  var s = make();
  promote();
  for (var round = 0; round < 3; round++) {
    var value = fresh(label + "#" + round);
    s.store(value);
    minor();
    check(label + " after store, round " + round, s.read, value);
    minor();
    minor();
    check(label + " after more collections, round " + round, s.read, value);
  }
}

scenario("assignment to var", function () {
  var held = fresh("initial");
  return { store: function (v) { held = v; }, read: function () { return held; } };
});

scenario("assignment to let", function () {
  let held = fresh("initial");
  return { store: function (v) { held = v; }, read: function () { return held; } };
});

scenario("array destructuring assignment", function () {
  var held = fresh("initial");
  return { store: function (v) { [held] = [v]; }, read: function () { return held; } };
});

scenario("object destructuring assignment", function () {
  var held = fresh("initial");
  return { store: function (v) { ({ held } = { held: v }); }, read: function () { return held; } };
});

scenario("for-of target", function () {
  var held = fresh("initial");
  return {
    store: function (v) { for (held of [v]) {} },
    read: function () { return held; },
  };
});

scenario("class binding initialized after closure creation", function () {
  function build() {
    var read = function () { return C; };
    var store = function (v) { C = v; };
    class C {}
    return { store: store, read: read };
  }
  return build();
});

scenario("loop-iteration binding", function () {
  var cells = [];
  for (let i = fresh("initial"); cells.length < 3; i = null) {
    cells.push({ store: function (v) { i = v; }, read: function () { return i; } });
  }
  var pick = cells[1];
  return pick;
});

scenario("generator local", function () {
  function* g() {
    var held = fresh("initial");
    var reader = function () { return held; };
    while (true) held = yield reader;
  }
  var it = g();
  var read = it.next().value;
  return { store: function (v) { it.next(v); }, read: read };
});

// Derived constructor: an arrow created before super() reads the `this` that
// super() later binds in the constructor's environment.
(function () {
  var seen;
  class Base {
    constructor() {
      promote();
    }
  }
  class Derived extends Base {
    constructor() {
      var arrow = () => this;
      super();
      seen = arrow;
      minor();
      minor();
    }
  }
  var instance = new Derived();
  minor();
  assert.sameValue(seen(), instance, "this bound by super() is the instance");
  minor();
  assert.sameValue(seen(), instance, "this survives further collections");
})();

// Promise reactions run later through handlers that are old by then.
var pending = [];
var promiseHeld = fresh("initial");
var promiseRead = function () { return promiseHeld; };
for (var i = 0; i < 4; i++) {
  pending.push(Promise.resolve(i).then(function (n) { return n; }));
}

promote();

var asyncLabel = "async";
var asyncValues = [];
async function asyncScenario() {
  var held = fresh("initial");
  var read = function () { return held; };
  promote();
  for (var round = 0; round < 3; round++) {
    var value = fresh(asyncLabel + "#" + round);
    held = await Promise.resolve(value);
    minor();
    check("async local after await, round " + round, read, value);
    minor();
    check("async local after more collections, round " + round, read, value);
  }
}

Promise.all(pending)
  .then(function () {
    var value = fresh("promise reaction");
    return Promise.resolve(value).then(function (v) {
      promiseHeld = v;
      minor();
      minor();
      check("promise reaction store", promiseRead, value);
    });
  })
  .then(asyncScenario)
  .then($DONE, $DONE);
