/*---
description: Member References retain operands across collecting accessors and coercions
esid: sec-property-accessors-runtime-semantics-evaluation
features: [class, class-fields-private, class-methods-private]
---*/

var failure = {};
var throwing = false;
var stored;
var keyCalls = 0;

function collect() {
  $262.gc();
  if (throwing) throw failure;
}

function pending() {
  return { marker: 17 };
}

function observe(older, value) {
  assert.sameValue(older.marker, 17);
  return value;
}

var key = {
  toString: function () {
    keyCalls++;
    collect();
    return "value";
  }
};

var proto = {
  get value() {
    collect();
    assert.sameValue(this.marker, 31);
    return { valueOf: function () { collect(); return 23; } };
  },
  set value(value) {
    collect();
    assert.sameValue(this.marker, 31);
    stored = value;
  },
  get method() {
    collect();
    return function () { collect(); return this.marker; };
  }
};

function base() {
  var value = Object.create(proto);
  value.marker = 31;
  return value;
}

function namedGet() { return observe(pending(), base().value); }
function computedGet() { return observe(pending(), base()[key]); }
function namedCall() { return observe(pending(), base().method()); }
function computedCall() { return observe(pending(), base()["method"]()); }
function namedSet() { return observe(pending(), base().value = { marker: 41 }); }
function computedSet() { return observe(pending(), base()[key] = { marker: 42 }); }
function namedUpdate() { return observe(pending(), base().value++); }
function computedUpdate() { return observe(pending(), base()[key]++); }
function computedCompound() { return observe(pending(), base()[key] += 1); }

assert.sameValue(+namedGet(), 23);
assert.sameValue(+computedGet(), 23);
assert.sameValue(namedCall(), 31);
assert.sameValue(computedCall(), 31);
assert.sameValue(namedSet().marker, 41);
assert.sameValue(stored.marker, 41);
assert.sameValue(computedSet().marker, 42);
assert.sameValue(stored.marker, 42);
assert.sameValue(namedUpdate(), 23);
assert.sameValue(stored, 24);
assert.sameValue(computedUpdate(), 23);
assert.sameValue(stored, 24);
assert.sameValue(keyCalls, 3, "computed update coerces its key once");
assert.sameValue(computedCompound(), 24);
assert.sameValue(stored, 24);
assert.sameValue(keyCalls, 4, "compound assignment coerces its key once");

class PrivateAccess {
  #stored;
  get #value() { collect(); return 23; }
  set #value(value) { collect(); this.#stored = value; }
  get #method() {
    collect();
    return function () { collect(); return this.#value; };
  }
  read() { return observe(pending(), this.#value); }
  call() { return observe(pending(), this.#method()); }
  write() { return observe(pending(), this.#value = { marker: 43 }); }
  update() { return observe(pending(), this.#value++); }
  stored() { return this.#stored; }
}

assert.sameValue(new PrivateAccess().read(), 23);
assert.sameValue(new PrivateAccess().call(), 23);
assert.sameValue(new PrivateAccess().write().marker, 43);
var privateBase = new PrivateAccess();
assert.sameValue(privateBase.update(), 23);
assert.sameValue(privateBase.stored(), 24);

throwing = true;
var operations = [
  namedGet, computedGet, namedCall, computedCall,
  namedSet, computedSet, namedUpdate, computedUpdate, computedCompound,
  function () { return new PrivateAccess().read(); },
  function () { return new PrivateAccess().call(); },
  function () { return new PrivateAccess().write(); },
  function () { return new PrivateAccess().update(); }
];
for (var i = 0; i < operations.length; i++) {
  var caught;
  try {
    operations[i]();
  } catch (error) {
    caught = error;
  }
  assert.sameValue(caught, failure, "original collecting error " + i);
}

throwing = false;
// A nullish base throws before the key's observable coercion.
keyCalls = 0;
assert.throws(TypeError, function () { return null[key] += 1; });
assert.sameValue(keyCalls, 0);
