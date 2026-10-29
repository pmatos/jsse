// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-object.prototype.tostring
description: >
  Object.prototype.toString reports builtinTag "Error" for AggregateError and
  SuppressedError instances, which have an [[ErrorData]] internal slot.
info: |
  20.1.3.6 Object.prototype.toString ( )
    ...
    9. Else if O has an [[ErrorData]] internal slot, let builtinTag be "Error".
    ...
    16. Return the string-concatenation of "[object ", tag, and "]".

  20.5.7.1.1 AggregateError ( errors, message [ , options ] )
    2. Let O be ? OrdinaryCreateFromConstructor(newTarget, "%AggregateError.prototype%", « [[ErrorData]] »).

  20.5.8.1.1 SuppressedError ( error, suppressed, message )
    3. Let O be ? OrdinaryCreateFromConstructor(newTarget, "%SuppressedError.prototype%", « [[ErrorData]] »).
features: [AggregateError, explicit-resource-management, Symbol.toStringTag, class]
---*/

var toString = Object.prototype.toString;

assert.sameValue(toString.call(new AggregateError([])), "[object Error]", "new AggregateError");
assert.sameValue(toString.call(AggregateError([])), "[object Error]", "AggregateError called as a function");
assert.sameValue(toString.call(new SuppressedError(1, 2)), "[object Error]", "new SuppressedError");
assert.sameValue(toString.call(SuppressedError(1, 2)), "[object Error]", "SuppressedError called as a function");

class MyAggregate extends AggregateError {}
class MySuppressed extends SuppressedError {}
assert.sameValue(toString.call(new MyAggregate([])), "[object Error]", "AggregateError subclass");
assert.sameValue(toString.call(new MySuppressed(1, 2)), "[object Error]", "SuppressedError subclass");

assert.sameValue(
  toString.call(Reflect.construct(AggregateError, [[]], Object)),
  "[object Error]",
  "AggregateError with unrelated newTarget"
);

assert.sameValue(toString.call(AggregateError.prototype), "[object Object]", "AggregateError.prototype has no [[ErrorData]]");
assert.sameValue(toString.call(SuppressedError.prototype), "[object Object]", "SuppressedError.prototype has no [[ErrorData]]");

var agg = new AggregateError([]);
agg[Symbol.toStringTag] = "Custom";
assert.sameValue(toString.call(agg), "[object Custom]", "@@toStringTag overrides builtinTag");

var sup = new SuppressedError(1, 2);
Object.defineProperty(sup, Symbol.toStringTag, { value: 1 });
assert.sameValue(toString.call(sup), "[object Error]", "non-string @@toStringTag is ignored");
