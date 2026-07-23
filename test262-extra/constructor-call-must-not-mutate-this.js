/*---
description: >
  Built-in "constructor" functions that box a primitive or allocate a fresh
  error object (Boolean, Number, String, Error and the NativeError subtypes,
  AggregateError) must ignore the passed-in `this` value when invoked via
  [[Call]] (NewTarget undefined) — e.g. through Function.prototype.call/apply,
  or as a borrowed method with an explicit thisArg. They must never read or
  write internal slots (or the [[Prototype]]) of an arbitrary `this` object,
  only ever act on a fresh object they allocate via
  OrdinaryCreateFromConstructor when actually invoked via [[Construct]].
info: |
  21.3.1.1 Boolean ( value )
    2. If NewTarget is undefined, return b.
    3. Let O be ? OrdinaryCreateFromConstructor(NewTarget, "%Boolean.prototype%", « [[BooleanData]] »).

  21.1.1.1 Number ( value )
    3. If NewTarget is undefined, return n.
    4. Let O be ? OrdinaryCreateFromConstructor(NewTarget, "%Number.prototype%", « [[NumberData]] »).

  22.1.1.1 String ( value )
    3. If NewTarget is undefined, return s.
    4. Let O be ? OrdinaryCreateFromConstructor(NewTarget, "%String.prototype%", « [[StringData]] »).

  20.5.1.1 Error ( message [ , options ] )
    1. If NewTarget is undefined, let newTarget be the active function object;
       else let newTarget be NewTarget.
    2. Let O be ? OrdinaryCreateFromConstructor(newTarget, "%Error.prototype%", « [[ErrorData]] »).

  20.5.6.1.1 NativeError ( message [ , options ] ) and 20.5.7.1 AggregateError
  follow the same OrdinaryCreateFromConstructor shape as Error — a fresh
  object, never the incoming `this`.

  None of these abstract operations read from or write to an incoming `this`
  value; they only ever construct and return a brand-new ordinary object.
esid: sec-boolean-constructor-boolean-value
---*/

function assertNotBoxed(obj, protoBefore, label) {
  if (Object.getPrototypeOf(obj) !== protoBefore) {
    throw new Test262Error(
      label + ' must not change the [[Prototype]] of an arbitrary `this`, got ' +
      Object.getPrototypeOf(obj)
    );
  }
  if (obj.hasOwnProperty('message') || obj.hasOwnProperty('errors') ||
      obj.hasOwnProperty('cause')) {
    throw new Test262Error(label + ' must not add own properties to an arbitrary `this`');
  }
}

// --- Boolean ---
{
  var obj = {};
  var protoBefore = Object.getPrototypeOf(obj);
  var result = Boolean.call(obj, 1);
  assertNotBoxed(obj, protoBefore, 'Boolean.call');
  assert.sameValue(result, true, 'Boolean.call still performs ToBoolean and returns a primitive');
  assert.sameValue(typeof result, 'boolean');
}

// --- Number ---
{
  var obj = {};
  var protoBefore = Object.getPrototypeOf(obj);
  var result = Number.call(obj, 5);
  assertNotBoxed(obj, protoBefore, 'Number.call');
  assert.sameValue(result, 5, 'Number.call still performs ToNumber and returns a primitive');
  assert.sameValue(typeof result, 'number');
}

// --- String ---
{
  var obj = {};
  var protoBefore = Object.getPrototypeOf(obj);
  var result = String.call(obj, 'x');
  assertNotBoxed(obj, protoBefore, 'String.call');
  assert.sameValue(result, 'x', 'String.call still performs ToString and returns a primitive');
  assert.sameValue(typeof result, 'string');
  assert.sameValue(
    Object.prototype.toString.call(obj),
    '[object Object]',
    'String.call must not tag an arbitrary `this` as a boxed String'
  );
}

// --- Error and NativeError subtypes ---
[Error, TypeError, RangeError, ReferenceError, SyntaxError, EvalError, URIError].forEach(
  function (Ctor) {
    var obj = {};
    var protoBefore = Object.getPrototypeOf(obj);
    var result = Ctor.call(obj, 'boom');
    assertNotBoxed(obj, protoBefore, Ctor.name + '.call');
    if (!(result instanceof Ctor)) {
      throw new Test262Error(Ctor.name + '.call(this, msg) must still return a fresh ' + Ctor.name);
    }
    assert.sameValue(result.message, 'boom');
    if (result === obj) {
      throw new Test262Error(Ctor.name + '.call must return a new object, not the passed `this`');
    }
  }
);

// --- AggregateError ---
{
  var obj = {};
  var protoBefore = Object.getPrototypeOf(obj);
  var result = AggregateError.call(obj, [], 'boom');
  assertNotBoxed(obj, protoBefore, 'AggregateError.call');
  if (!(result instanceof AggregateError)) {
    throw new Test262Error('AggregateError.call(this, errors, msg) must still return a fresh AggregateError');
  }
  assert.sameValue(result.message, 'boom');
}

// --- The exact reported repro: a class method with a defaulted second
// parameter forwarding a native function via fn.call(thisArg, ...) must not
// corrupt `thisArg` (or the object arena in a way that affects later calls). ---
{
  class C {
    m(fn, thisArg = this) {
      return fn.call(thisArg, 1);
    }
  }
  var c = new C();
  assert.sameValue(c.m(Boolean), true, 'first call, boxed via a native function, returns correctly');
  assert.sameValue(
    Object.getPrototypeOf(c),
    C.prototype,
    'the receiver`s [[Prototype]] must survive a native-function first call'
  );
  assert.sameValue(
    c.m(function (x) { return x + 1; }),
    2,
    'a later call on the same method must still work'
  );
}
