/*---
description: >
  An object rest element following a property whose default suspends --
  whether on `await` or on `yield` -- is lowered in an async generator body
  just as in a plain async function or a synchronous generator: each
  suspends the async generator at its own point and the rest object
  excludes every key already consumed by an earlier property (issue #771).
esid: sec-destructuring-binding-patterns-runtime-semantics-restbindinginitialization
info: |
  RestBindingInitialization

  BindingRestProperty : ... BindingIdentifier

  1. Let lhs be ? ResolveBinding(...)
  2. Let restObj be OrdinaryObjectCreate(%Object.prototype%).
  3. Perform ? CopyDataProperties(restObj, value, excludedNames).
  4. Return ? InitializeReferencedBinding(lhs, restObj).
flags: [async]
features: [async-iteration, destructuring-binding, object-rest]
---*/

async function run() {
  async function* g() {
    var { a = await 1, ...rest } = { c: 3 };
    yield { a: a, rest: rest };
    var { b = yield 2, ...rest2 } = { d: 4 };
    return { b: b, rest2: rest2 };
  }
  var it = g();
  var r1 = await it.next();
  var r2 = await it.next();
  var r3 = await it.next(100);
  return [r1, r2, r3];
}

run()
  .then(function (results) {
    var r1 = results[0];
    var r2 = results[1];
    var r3 = results[2];

    assert.sameValue(r1.done, false, 'the first yield has not completed the generator');
    assert.sameValue(r1.value.a, 1, "the await default's suspension resolves before the yield");
    assert.sameValue(r1.value.rest.c, 3, 'rest keeps properties not named by the pattern');

    assert.sameValue(r2.done, false, "the second property's own yield default suspends");
    assert.sameValue(r2.value, 2, 'the yield default value reaches the caller');

    assert.sameValue(r3.done, true, 'the generator completes after the sent value is used');
    assert.sameValue(r3.value.b, 100, 'the value sent to .next() is used for the yield default');
    assert.sameValue(r3.value.rest2.d, 4, 'the second rest keeps properties not named by its pattern');
  })
  .then($DONE, $DONE);
