// Copyright (C) 2026 jsse contributors. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
description: >
  Reads and writes of a private accessor member go through the PrivateGet and
  PrivateSet abstract operations regardless of the surrounding expression form.
  A read of an accessor that has no getter throws a TypeError; a write whose
  setter throws propagates that throw; a logical assignment (&&=, ||=, ??=), a
  compound assignment (+=, -=, ...), and an update (++, --) all desugar to the
  same PrivateGet/PrivateSet pair, so an accessor-backed private behaves exactly
  as a data-field private would through each of those forms.
esid: sec-privateget
features: [class, class-fields-private, class-methods-private, BigInt]
info: |
  7.3.28 PrivateGet ( O, P )
    3. Assert: P.[[Kind]] is accessor.
    3.a. If P does not have a [[Get]] field, throw a TypeError exception.
    3.b. Let getter be P.[[Get]].
    3.c. Return ? Call(getter, O).

  7.3.29 PrivateSet ( O, P, value )
    4. Else, Assert: P.[[Kind]] is accessor.
    4.a. If P does not have a [[Set]] field, throw a TypeError exception.
    4.b. Let setter be P.[[Set]].
    4.c. Perform ? Call(setter, O, << value >>).

  13.15.2 AssignmentExpression : LeftHandSideExpression {&&=,||=,??=} ...
    Evaluates the reference, GetValue (PrivateGet) it, short-circuits on the
    boolean/nullish test, and only then PutValue (PrivateSet).
---*/

// ---------------------------------------------------------------------------
// PrivateGet on an accessor with no getter throws a TypeError, and the throw
// surfaces through every logical-assignment operator that must read first.
// ---------------------------------------------------------------------------

class SetOnly {
  set #x(v) {}
  static nullish(o) {
    return o.#x ??= 1;
  }
  static or(o) {
    return o.#x ||= 1;
  }
  static and(o) {
    return o.#x &&= 1;
  }
}

assert.throws(
  TypeError,
  function () {
    SetOnly.nullish(new SetOnly());
  },
  "??= reads the set-only accessor and PrivateGet throws"
);

assert.throws(
  TypeError,
  function () {
    SetOnly.or(new SetOnly());
  },
  "||= reads the set-only accessor and PrivateGet throws"
);

assert.throws(
  TypeError,
  function () {
    SetOnly.and(new SetOnly());
  },
  "&&= reads the set-only accessor and PrivateGet throws"
);

// Control: a get-only accessor whose value is non-nullish short-circuits the
// assignment, so the getter's value is returned and no setter is needed.
class GetOnly {
  get #x() {
    return 7;
  }
  static nullish(o) {
    return o.#x ??= 1;
  }
}

assert.sameValue(
  GetOnly.nullish(new GetOnly()),
  7,
  "??= on a non-nullish accessor read short-circuits without assigning"
);

// ---------------------------------------------------------------------------
// When a logical assignment does reach the write, PrivateSet invokes the
// setter, and an abrupt completion from the setter propagates (it must not be
// swallowed). A distinct error subclass proves it is the setter's throw.
// ---------------------------------------------------------------------------

class SetterThrew extends Error {}

class ThrowingSetter {
  #v;
  constructor(v) {
    this.#v = v;
  }
  get #x() {
    return this.#v;
  }
  set #x(value) {
    throw new SetterThrew("setter ran");
  }
  static or(o) {
    return o.#x ||= 1;
  }
  static and(o) {
    return o.#x &&= 1;
  }
  static nullish(o) {
    return o.#x ??= 1;
  }
}

assert.throws(
  SetterThrew,
  function () {
    // getter returns 0 (falsy) -> ||= assigns -> setter throws
    ThrowingSetter.or(new ThrowingSetter(0));
  },
  "||= assigns through the setter and the setter's throw propagates"
);

assert.throws(
  SetterThrew,
  function () {
    // getter returns 1 (truthy) -> &&= assigns -> setter throws
    ThrowingSetter.and(new ThrowingSetter(1));
  },
  "&&= assigns through the setter and the setter's throw propagates"
);

assert.throws(
  SetterThrew,
  function () {
    // getter returns null (nullish) -> ??= assigns -> setter throws
    ThrowingSetter.nullish(new ThrowingSetter(null));
  },
  "??= assigns through the setter and the setter's throw propagates"
);

// Control: a successful setter records the assigned value and the assignment
// expression evaluates to the assigned value.
class RecordingSetter {
  #v = 0;
  get #x() {
    return this.#v;
  }
  set #x(value) {
    this.#v = value;
  }
  static or(o) {
    return o.#x ||= 5;
  }
  static read(o) {
    return o.#x;
  }
}

var rec = new RecordingSetter();
assert.sameValue(
  RecordingSetter.or(rec),
  5,
  "||= through a setter evaluates to the assigned value"
);
assert.sameValue(
  RecordingSetter.read(rec),
  5,
  "||= through a setter updates the backing field"
);

// ---------------------------------------------------------------------------
// An update expression (++ / --) on an accessor-backed private desugars to
// PrivateGet -> ToNumeric -> PrivateSet, exactly as for a data field: the
// getter supplies the operand, ToNumeric coerces it, and the setter receives
// the incremented/decremented value.
// ---------------------------------------------------------------------------

class Counter {
  #v;
  constructor(v) {
    this.#v = v;
  }
  get #x() {
    return this.#v;
  }
  set #x(value) {
    this.#v = value;
  }
  static postInc(o) {
    return o.#x++;
  }
  static preDec(o) {
    return --o.#x;
  }
  static read(o) {
    return o.#x;
  }
}

var c = new Counter(5);
assert.sameValue(Counter.postInc(c), 5, "o.#x++ evaluates to the old value");
assert.sameValue(Counter.read(c), 6, "o.#x++ writes the incremented value via the setter");
assert.sameValue(Counter.preDec(c), 5, "--o.#x evaluates to the new value");
assert.sameValue(Counter.read(c), 5, "--o.#x writes the decremented value via the setter");

// ToNumeric is applied to the getter result: a string getter yields a Number.
class StringGetter {
  last;
  get #x() {
    return "5";
  }
  set #x(value) {
    this.last = value;
  }
  static postInc(o) {
    return o.#x++;
  }
}

var sg = new StringGetter();
var sgResult = StringGetter.postInc(sg);
assert.sameValue(typeof sgResult, "number", "o.#x++ result is ToNumeric-coerced");
assert.sameValue(sgResult, 5, 'o.#x++ coerces the "5" getter result to 5');
assert.sameValue(sg.last, 6, "the setter receives the incremented Number");

// BigInt operands stay BigInt through the update.
class BigCounter {
  #v = 5n;
  get #x() {
    return this.#v;
  }
  set #x(value) {
    this.#v = value;
  }
  static postInc(o) {
    return o.#x++;
  }
  static read(o) {
    return o.#x;
  }
}

var bc = new BigCounter();
assert.sameValue(BigCounter.postInc(bc), 5n, "o.#x++ evaluates to the old BigInt");
assert.sameValue(BigCounter.read(bc), 6n, "o.#x++ increments the BigInt via the setter");

// An update through an accessor with no getter throws (PrivateGet step 3.a).
class SetOnlyUpdate {
  set #x(value) {}
  static postInc(o) {
    return o.#x++;
  }
}

assert.throws(
  TypeError,
  function () {
    SetOnlyUpdate.postInc(new SetOnlyUpdate());
  },
  "o.#x++ reads the set-only accessor first and PrivateGet throws"
);

// ---------------------------------------------------------------------------
// A compound assignment (+=, -=, ...) on a private reference also desugars to
// PrivateGet -> op -> PrivateSet, so it reads through the getter. A plain
// assignment (=) performs no PrivateGet, so a set-only accessor accepts it.
// ---------------------------------------------------------------------------

// Compound assignment reads the accessor, so a set-only accessor throws.
class SetOnlyCompound {
  set #x(value) {}
  static addAssign(o) {
    return o.#x += 1;
  }
}

assert.throws(
  TypeError,
  function () {
    SetOnlyCompound.addAssign(new SetOnlyCompound());
  },
  "o.#x += 1 reads the set-only accessor first and PrivateGet throws"
);

// Plain assignment does NOT read, so the same set-only accessor accepts `=`.
class SetOnlyPlain {
  last;
  set #x(value) {
    this.last = value;
  }
  static assign(o) {
    return o.#x = 9;
  }
}

var sop = new SetOnlyPlain();
assert.sameValue(
  SetOnlyPlain.assign(sop),
  9,
  "o.#x = v evaluates to the assigned value with no PrivateGet"
);
assert.sameValue(sop.last, 9, "plain assignment reaches the setter");

// Compound assignment round-trips through getter and setter.
class Accumulator {
  #v = 10;
  get #x() {
    return this.#v;
  }
  set #x(value) {
    this.#v = value;
  }
  static addAssign(o) {
    return o.#x += 5;
  }
  static read(o) {
    return o.#x;
  }
}

var acc = new Accumulator();
assert.sameValue(Accumulator.addAssign(acc), 15, "o.#x += 5 evaluates to the computed value");
assert.sameValue(Accumulator.read(acc), 15, "o.#x += 5 writes the computed value via the setter");

// A throwing setter reached through a compound assignment propagates.
class CompoundThrows {
  get #x() {
    return 0;
  }
  set #x(value) {
    throw new SetterThrew("compound setter ran");
  }
  static addAssign(o) {
    return o.#x += 1;
  }
}

assert.throws(
  SetterThrew,
  function () {
    CompoundThrows.addAssign(new CompoundThrows());
  },
  "o.#x += 1 assigns through the setter and its throw propagates"
);

