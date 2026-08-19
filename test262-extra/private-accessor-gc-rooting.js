/*---
description: >
  A private reference's receiver stays reachable while an update expression
  coerces the value its getter returned, and while a logical assignment
  evaluates its right-hand side.
esid: sec-privateset
features: [class, class-methods-private]
info: |
  13.4.2.1 Runtime Semantics: Evaluation (UpdateExpression)
    1. Let expr be ? Evaluation of UnaryExpression.
    2. Let oldValue be ? ToNumeric(? GetValue(expr)).
    ...
    5. Perform ? PutValue(expr, newValue).

  13.15.2 Runtime Semantics: Evaluation
      (AssignmentExpression : LeftHandSideExpression &&= / ||= / ??= AssignmentExpression)
    1. Let lref be ? Evaluation of LeftHandSideExpression.
    2. Let lval be ? GetValue(lref).
    ...
    5. Perform ? PutValue(lref, rval).

  In both forms the Reference produced in step 1 must survive until PutValue
  runs, even though the intervening steps call user code: ToNumeric invokes
  valueOf, and the right-hand side of a logical assignment is arbitrary. When
  the base of that Reference is a temporary object -- `(new C()).#x++` -- an
  implementation that holds it only in a native local can collect it during
  those calls and then skip PutValue, so the private setter never runs and any
  abrupt completion it would have produced is lost, even though the expression
  completes normally.
---*/

// A pre-existing coercible object, so only the receiver is unreachable when
// the collector runs.
var collectingCoercible = {
  valueOf: function () {
    $262.gc();
    return 1;
  },
};

function collectAndReturnSeven() {
  $262.gc();
  return 7;
}

// ---------------------------------------------------------------------------
// UpdateExpression: the getter hands back a coercible object whose valueOf
// collects, and PrivateSet must still reach the setter on the same receiver.
// ---------------------------------------------------------------------------

var postfixWrites = [];

class Postfix {
  get #x() {
    return collectingCoercible;
  }
  set #x(value) {
    postfixWrites.push(value);
  }
  static bump() {
    return (new Postfix()).#x++;
  }
}

assert.sameValue(Postfix.bump(), 1, "o.#x++ returns ToNumeric of the getter result");
assert.sameValue(postfixWrites.length, 1, "o.#x++ runs the setter exactly once");
assert.sameValue(postfixWrites[0], 2, "o.#x++ passes the incremented value to the setter");

var prefixWrites = [];

class Prefix {
  get #x() {
    return collectingCoercible;
  }
  set #x(value) {
    prefixWrites.push(value);
  }
  static bump() {
    return --(new Prefix()).#x;
  }
}

assert.sameValue(Prefix.bump(), 0, "--o.#x returns the decremented value");
assert.sameValue(prefixWrites.length, 1, "--o.#x runs the setter exactly once");
assert.sameValue(prefixWrites[0], 0, "--o.#x passes the decremented value to the setter");

// ---------------------------------------------------------------------------
// Logical assignment: the right-hand side collects between PrivateGet and
// PrivateSet, and the setter must still receive the value.
// ---------------------------------------------------------------------------

var nullishWrites = [];

class Nullish {
  get #x() {
    return undefined;
  }
  set #x(value) {
    nullishWrites.push(value);
  }
  static assign() {
    return (new Nullish()).#x ??= collectAndReturnSeven();
  }
}

assert.sameValue(Nullish.assign(), 7, "o.#x ??= rhs evaluates to the right-hand side");
assert.sameValue(nullishWrites.length, 1, "o.#x ??= rhs runs the setter exactly once");
assert.sameValue(nullishWrites[0], 7, "o.#x ??= rhs passes the right-hand side to the setter");

var orWrites = [];

class Or {
  get #x() {
    return false;
  }
  set #x(value) {
    orWrites.push(value);
  }
  static assign() {
    return (new Or()).#x ||= collectAndReturnSeven();
  }
}

assert.sameValue(Or.assign(), 7, "o.#x ||= rhs evaluates to the right-hand side");
assert.sameValue(orWrites.length, 1, "o.#x ||= rhs runs the setter exactly once");
assert.sameValue(orWrites[0], 7, "o.#x ||= rhs passes the right-hand side to the setter");

var andWrites = [];

class And {
  get #x() {
    return true;
  }
  set #x(value) {
    andWrites.push(value);
  }
  static assign() {
    return (new And()).#x &&= collectAndReturnSeven();
  }
}

assert.sameValue(And.assign(), 7, "o.#x &&= rhs evaluates to the right-hand side");
assert.sameValue(andWrites.length, 1, "o.#x &&= rhs runs the setter exactly once");
assert.sameValue(andWrites[0], 7, "o.#x &&= rhs passes the right-hand side to the setter");

// ---------------------------------------------------------------------------
// A throwing setter proves the write is not merely unobserved: PrivateSet's
// abrupt completion must still propagate out of the whole expression.
// ---------------------------------------------------------------------------

class SetterThrew extends Error {}

class ThrowingUpdate {
  get #x() {
    return collectingCoercible;
  }
  set #x(value) {
    throw new SetterThrew("update setter ran");
  }
  static bump() {
    return (new ThrowingUpdate()).#x++;
  }
}

assert.throws(
  SetterThrew,
  function () {
    ThrowingUpdate.bump();
  },
  "o.#x++ propagates a throw from the setter after a collecting coercion"
);

class ThrowingLogical {
  get #x() {
    return undefined;
  }
  set #x(value) {
    throw new SetterThrew("logical setter ran");
  }
  static assign() {
    return (new ThrowingLogical()).#x ??= collectAndReturnSeven();
  }
}

assert.throws(
  SetterThrew,
  function () {
    ThrowingLogical.assign();
  },
  "o.#x ??= rhs propagates a throw from the setter after a collecting rhs"
);
