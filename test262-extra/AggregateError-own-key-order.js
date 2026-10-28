// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-aggregate-error-constructor
description: >
  An AggregateError's own keys follow the order of the constructor's steps:
  message, then cause (InstallErrorCause), then errors.
info: |
  20.5.7.1.1 AggregateError ( errors, message [ , options ] )
    3. If message is not undefined, then
      b. Perform CreateNonEnumerableDataPropertyOrThrow(O, "message", msg).
    4. Perform ? InstallErrorCause(O, options).
    5. Let errorsList be ? IteratorToList(? GetIterator(errors, sync)).
    6. Perform ! DefinePropertyOrThrow(O, "errors", ...).
includes: [compareArray.js]
features: [AggregateError, error-cause, Reflect]
---*/

assert.compareArray(
  Reflect.ownKeys(new AggregateError([], "m")),
  ["message", "errors"],
  "message, errors"
);
assert.compareArray(
  Reflect.ownKeys(new AggregateError([1], "m", { cause: 0 })),
  ["message", "cause", "errors"],
  "message, cause, errors"
);
assert.compareArray(
  Reflect.ownKeys(AggregateError([], undefined, { cause: 0 })),
  ["cause", "errors"],
  "no message"
);
