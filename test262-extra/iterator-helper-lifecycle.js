/*---
description: >
  Lazy iterator helpers share terminal and executing states while retaining
  their individual yield, exhaustion, lazy-opening, and IteratorClose rules.
esid: sec-%iteratorhelperprototype%-object
info: |
  %IteratorHelperPrototype%.next delegates to GeneratorResume. An executing
  generator rejects reentry and a completed generator returns done without
  resuming its closure. GeneratorStart completes a closure after abrupt exit.

  %IteratorHelperPrototype%.return sets suspended-start to completed before
  IteratorCloseAll. Suspended-yield instead uses GeneratorResumeAbrupt.

  Iterator.prototype.take closes after its last ordinary Yield is resumed.
  Iterator.prototype.chunks and windows ignore the completion of their final
  partial Yield after IteratorStepValue has exhausted the source.

  Iterator.concat opens inputs lazily. Iterator.zip and zipKeyed close open
  iterator records in reverse order, retaining their mode-specific exhaustion.
includes: [compareArray.js]
features: [cross-realm, iterator-helpers, iterator-chunking, iterator-sequencing, joint-iteration]
---*/

function tracked(values, onNext, onReturn) {
  var record = { nextCalls: 0, returnCalls: 0 };
  var iterator = Object.create(Iterator.prototype);
  iterator.next = function () {
    var index = record.nextCalls++;
    if (onNext) onNext();
    return index < values.length
      ? { value: values[index], done: false }
      : { done: true };
  };
  iterator.return = function () {
    record.returnCalls++;
    if (onReturn) onReturn();
    return {};
  };
  record.iterator = iterator;
  return record;
}

var definitions = [
  { name: 'map', make: function (s) { return s.map(function (v) { return v; }); } },
  { name: 'filter', make: function (s) { return s.filter(function () { return true; }); } },
  { name: 'take', make: function (s) { return s.take(Infinity); } },
  { name: 'drop', make: function (s) { return s.drop(0); } },
  { name: 'chunks', make: function (s) { return s.chunks(1); } },
  { name: 'windows', make: function (s) { return s.windows(1); } },
  { name: 'flatMap', make: function (s) { return s.flatMap(function (v) { return [v]; }); } },
  { name: 'concat', make: function (s) { return Iterator.concat(s); } },
  { name: 'zip', make: function (s) { return Iterator.zip([s]); } },
  { name: 'zipKeyed', make: function (s) { return Iterator.zipKeyed({ a: s }); } }
];

function assertDone(result, message) {
  assert.sameValue(result.done, true, message);
  assert.sameValue(result.value, undefined, message);
}

for (var definition of definitions) {
  // Exhaustion is terminal and does not close the exhausted source.
  var source = tracked([1]);
  var helper = definition.make(source.iterator);
  assert.sameValue(helper.next().done, false, definition.name);
  assertDone(helper.next(), definition.name);
  var callsAtDone = source.nextCalls;
  assertDone(helper.next(), definition.name);
  assertDone(helper.return(123), definition.name);
  assertDone(helper.return(), definition.name);
  assert.sameValue(source.nextCalls, callsAtDone, definition.name);
  assert.sameValue(source.returnCalls, 0, definition.name);

  // next and return both reject reentry while a next body or a suspended
  // return is executing. Returning closes an open source exactly once.
  var reenter = function () {
    assert.throws(TypeError, function () { helper.next(); }, definition.name);
    assert.throws(TypeError, function () { helper.return(); }, definition.name);
  };
  source = tracked([1, 2], reenter, reenter);
  helper = definition.make(source.iterator);
  assert.sameValue(helper.next().done, false, definition.name);
  assertDone(helper.return(123), definition.name);
  assertDone(helper.next(), definition.name);
  assertDone(helper.return(), definition.name);
  assert.sameValue(source.nextCalls, 1, definition.name);
  assert.sameValue(source.returnCalls, 1, definition.name);

  // Before the body starts, completion is latched before the close callback.
  source = tracked([1], null, function () {
    assertDone(helper.next(), definition.name);
    assertDone(helper.return(), definition.name);
  });
  helper = definition.make(source.iterator);
  assertDone(helper.return(), definition.name);
  assertDone(helper.next(), definition.name);
  assert.sameValue(source.nextCalls, 0, definition.name);
  assert.sameValue(source.returnCalls, definition.name === 'concat' ? 0 : 1,
                   definition.name);

  // A throwing close is terminal, too; later operations never retry it.
  var closeError = new Test262Error(definition.name);
  source = tracked([1, 2], null, function () { throw closeError; });
  helper = definition.make(source.iterator);
  helper.next();
  var caught;
  try { helper.return(); } catch (error) { caught = error; }
  assert.sameValue(caught, closeError, definition.name);
  assertDone(helper.next(), definition.name);
  assertDone(helper.return(), definition.name);
  assert.sameValue(source.returnCalls, 1, definition.name);
}

// take's final value remains an ordinary suspension: close occurs on resume.
var taken = tracked([10, 20]);
var takeHelper = taken.iterator.take(1);
assert.sameValue(takeHelper.next().value, 10);
assert.sameValue(taken.returnCalls, 0);
assertDone(takeHelper.next());
assert.sameValue(taken.nextCalls, 1);
assert.sameValue(taken.returnCalls, 1);
assertDone(takeHelper.return());
assert.sameValue(taken.returnCalls, 1);

// A final partial chunk/window has already observed source exhaustion.
for (var partialMethod of ['chunks', 'windows']) {
  var partial = tracked([7]);
  var partialHelper = partialMethod === 'chunks'
    ? partial.iterator.chunks(2)
    : partial.iterator.windows(2, 'allow-partial');
  var partialResult = partialHelper.next();
  assert.sameValue(partialResult.done, false);
  assert.compareArray(partialResult.value, [7]);
  assert.sameValue(partial.nextCalls, 2);
  assertDone(partialHelper.return());
  assertDone(partialHelper.next());
  assert.sameValue(partial.nextCalls, 2);
  assert.sameValue(partial.returnCalls, 0);
}

var flatCloseLog = [];
var outer = tracked([1, 2], null, function () { flatCloseLog.push('outer'); });
var inner = tracked([10, 20], null, function () { flatCloseLog.push('inner'); });
var flatHelper = outer.iterator.flatMap(function () { return inner.iterator; });
assert.sameValue(flatHelper.next().value, 10);
assertDone(flatHelper.return());
assert.compareArray(flatCloseLog, ['inner', 'outer']);

// concat retrieves iterator methods at construction, but opens only on demand.
var concatLog = [];
function concatInput(name) {
  var input = {};
  Object.defineProperty(input, Symbol.iterator, {
    get: function () {
      concatLog.push('get:' + name);
      return function () {
        concatLog.push('open:' + name);
        return tracked([1, 2], null, function () {
          concatLog.push('close:' + name);
        }).iterator;
      };
    }
  });
  return input;
}
var concatHelper = Iterator.concat(concatInput('a'), concatInput('b'));
assert.compareArray(concatLog, ['get:a', 'get:b']);
assert.sameValue(concatHelper.next().value, 1);
assertDone(concatHelper.return());
assert.compareArray(concatLog, ['get:a', 'get:b', 'open:a', 'close:a']);

// A captured method must survive later @@iterator getters until the helper
// owns it. Neither source keeps the freshly returned method as a property.
var concatFirst = {
  get [Symbol.iterator]() {
    return function () { return [1].values(); };
  }
};
var concatSecond = {
  get [Symbol.iterator]() {
    $262.gc();
    return function () { return [2].values(); };
  }
};
var concatAfterGetterGc = Iterator.concat(concatFirst, concatSecond);
assert.sameValue(concatAfterGetterGc.next().value, 1);
assert.sameValue(concatAfterGetterGc.next().value, 2);
assertDone(concatAfterGetterGc.next());

function joint(keyed, a, b, options) {
  return keyed
    ? Iterator.zipKeyed({ a: a.iterator, b: b.iterator }, options)
    : Iterator.zip([a.iterator, b.iterator], options);
}
for (var keyed of [false, true]) {
  for (var mode of ['shortest', 'strict']) {
    var a = tracked([]);
    var b = tracked([2, 3]);
    var zipped = joint(keyed, a, b, { mode: mode });
    if (mode === 'strict') {
      assert.throws(TypeError, function () { zipped.next(); });
    } else {
      assertDone(zipped.next());
    }
    assertDone(zipped.next());
    assertDone(zipped.return());
    assert.sameValue(a.returnCalls, 0);
    assert.sameValue(b.returnCalls, 1);
  }

  a = tracked([1]);
  b = tracked([2, 3]);
  zipped = joint(keyed, a, b, {
    mode: 'longest', padding: keyed ? { a: 9, b: 8 } : [9, 8]
  });
  zipped.next();
  var padded = zipped.next();
  assert.sameValue(padded.done, false);
  if (keyed) {
    assert.sameValue(padded.value.a, 9);
    assert.sameValue(padded.value.b, 3);
  } else {
    assert.compareArray(padded.value, [9, 3]);
  }
  assertDone(zipped.return());
  assert.sameValue(a.returnCalls, 0);
  assert.sameValue(b.returnCalls, 1);

  var jointCloseLog = [];
  a = tracked([1, 2], null, function () { jointCloseLog.push('a'); });
  b = tracked([3, 4], null, function () { jointCloseLog.push('b'); });
  zipped = joint(keyed, a, b);
  zipped.next();
  assertDone(zipped.return());
  assert.compareArray(jointCloseLog, ['b', 'a']);
}

// Borrowed prototype methods run in their own Realm, while resuming a helper
// closure runs in the Realm where that helper was created.
var otherGlobal = $262.createRealm().global;
var otherHelper = otherGlobal.Iterator.prototype.map.call(
  tracked([]).iterator, function (value) { return value; }
);
var otherHelperPrototype = otherGlobal.Object.getPrototypeOf(otherHelper);
var otherNext = otherHelperPrototype.next;
var otherReturn = otherHelperPrototype.return;

function assertResultRealm(result, prototype, message) {
  assertDone(result, message);
  assert.sameValue(Object.getPrototypeOf(result), prototype, message);
}

for (var realmDefinition of definitions) {
  var realmHelper = realmDefinition.make(tracked([]).iterator);
  assertDone(realmHelper.next());
  assertResultRealm(otherNext.call(realmHelper), otherGlobal.Object.prototype,
                    realmDefinition.name + ' completed next');
  assertResultRealm(otherReturn.call(realmHelper), otherGlobal.Object.prototype,
                    realmDefinition.name + ' completed return');

  var realmReenter = function () {
    assert.throws(otherGlobal.TypeError, function () {
      otherNext.call(realmHelper);
    }, realmDefinition.name + ' borrowed next while executing');
    assert.throws(otherGlobal.TypeError, function () {
      otherReturn.call(realmHelper);
    }, realmDefinition.name + ' borrowed return while executing');
  };
  var realmSource = tracked([1, 2], realmReenter, realmReenter);
  realmHelper = realmDefinition.make(realmSource.iterator);
  assert.sameValue(realmHelper.next().done, false);
  assertDone(realmHelper.return());
  assert.sameValue(realmSource.returnCalls, 1);
}

// The ignored-completion final Yield is still a suspension, even though the
// source is exhausted. Its first resume enters the original closure's Realm;
// only subsequent completed operations allocate in the borrowed method's Realm.
for (var realmPartialMethod of ['chunks', 'windows']) {
  for (var borrowedResume of [otherNext, otherReturn]) {
    var realmPartial = tracked([7]);
    var realmPartialHelper = realmPartialMethod === 'chunks'
      ? realmPartial.iterator.chunks(2)
      : realmPartial.iterator.windows(2, 'allow-partial');
    assert.sameValue(realmPartialHelper.next().done, false);
    assertResultRealm(borrowedResume.call(realmPartialHelper), Object.prototype,
                      realmPartialMethod + ' first final-yield resume');
    assertResultRealm(otherNext.call(realmPartialHelper), otherGlobal.Object.prototype,
                      realmPartialMethod + ' completed next');
    assertResultRealm(otherReturn.call(realmPartialHelper), otherGlobal.Object.prototype,
                      realmPartialMethod + ' completed return');
    assert.sameValue(realmPartial.nextCalls, 2);
    assert.sameValue(realmPartial.returnCalls, 0);
  }
}

// REGRESSION: a throwing value getter terminates the helper closure.
// IteratorStepValue marks the source done on failure, so it is not closed.
for (var failingName of ['map', 'filter', 'take', 'drop', 'flatMap', 'concat']) {
  var failingDefinition;
  for (var candidate of definitions) {
    if (candidate.name === failingName) failingDefinition = candidate;
  }
  var nextCalls = 0;
  var valueCalls = 0;
  var closeCalls = 0;
  var valueError = new Test262Error(failingName);
  var failingSource = Object.create(Iterator.prototype);
  failingSource.next = function () {
    nextCalls++;
    return {
      done: false,
      get value() { valueCalls++; throw valueError; }
    };
  };
  failingSource.return = function () { closeCalls++; return {}; };
  var failingHelper = failingDefinition.make(failingSource);
  caught = undefined;
  try { failingHelper.next(); } catch (error) { caught = error; }
  assert.sameValue(caught, valueError, failingName);
  assertDone(failingHelper.next(), failingName);
  assertDone(failingHelper.return(), failingName);
  assert.sameValue(nextCalls, 1, failingName);
  assert.sameValue(valueCalls, 1, failingName);
  assert.sameValue(closeCalls, 0, failingName);
}
