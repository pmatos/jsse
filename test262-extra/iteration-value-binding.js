/*---
description: Iteration values bind and assign consistently across execution paths
esid: sec-for-in-and-for-of-statements
info: |
  ForIn/OfBodyEvaluation selects BindingInitialization or PutValue for each
  iteration value. Lexical declarations create per-iteration bindings;
  assignment targets use existing bindings. Binding failures precede the
  body and close a for-of iterator without replacing an existing throw.
  Using declarations register the resource before initializing the binding
  and dispose it when that iteration ends.
flags: [async]
includes: [compareArray.js]
features: [generators, async-functions, async-iteration, destructuring-binding, destructuring-assignment, explicit-resource-management]
---*/

function valuesOf(readers) {
  return readers.map(function (read) { return read(); });
}

function single(value, log, closeError) {
  var used = false;
  return {
    [Symbol.iterator]: function () { return this; },
    next: function () {
      log.push('next');
      if (used) return { done: true };
      used = true;
      return { value: value, done: false };
    },
    return: function () {
      log.push('close');
      if (closeError !== undefined) throw closeError;
      return { done: true };
    }
  };
}

function resource(log, name) {
  return {
    name: name,
    get [Symbol.dispose]() {
      log.push('get ' + name);
      return function () { log.push('dispose ' + this.name); };
    }
  };
}

// Without eval or suspension this function can use native VM simple heads,
// complex BindHead instructions and the distinct catch-binding instruction.
function ordinaryHeads() {
  var readers = [];
  for (var shared of [1, 2]) readers.push(function () { return shared; });
  assert.compareArray(valuesOf(readers), [2, 2], 'var has one binding');
  readers = [];
  for (let own of [1, 2]) readers.push(function () { return own; });
  assert.compareArray(valuesOf(readers), [1, 2], 'let has per-iteration bindings');
  readers = [];
  for (const own in { a: 0, b: 0 }) readers.push(function () { return own; });
  assert.compareArray(valuesOf(readers), ['a', 'b'], 'for-in lexical captures');
  var assigned;
  for (assigned in { a: 0, b: 0 }) {}
  assert.sameValue(assigned, 'b', 'for-in assigns the existing binding');
  for (var { value: named } of [{ value: 3 }]) {}
  assert.sameValue(named, 3, 'var destructuring reaches the function binding');
  var log = [];
  var input = { get value() { log.push('get'); return undefined; } };
  function fallback() { log.push('default'); return 4; }
  for (let { value: local = fallback() } of [input]) {
    log.push('body ' + local);
  }
  for ({ value: assigned = fallback() } of [input]) {
    log.push('assigned ' + assigned);
  }
  assert.compareArray(log, ['get', 'default', 'body 4', 'get', 'default', 'assigned 4']);
  log = [];
  var target = { set value(v) { log.push('set ' + v); } };
  function base() { log.push('base'); return target; }
  function key() { log.push('key'); return 'value'; }
  for (base()[key()] of [5]) log.push('body');
  assert.compareArray(log, ['base', 'key', 'set 5', 'body']);
  var caught;
  try { throw { value: 6 }; } catch ({ value }) { caught = value; }
  assert.sameValue(caught, 6, 'catch keeps its own binding path');
}

ordinaryHeads();

// Top-level ordinary loops retain the same simple-head behavior in either
// execution mode, including a reused iteration environment when safe.
var total = 0;
for (let simple of [1, 2, 3]) total += simple;
for (const simple in { a: 1, b: 2 }) total += simple.length;
assert.sameValue(total, 8);

var original = {};
var closing = {};
var log = [];
function failBinding() { log.push('default'); throw original; }
var caught;
try {
  for (let { value = failBinding() } of single({}, log, closing)) {
    log.push('unreachable');
  }
} catch (error) { caught = error; }
assert.sameValue(caught, original, 'the binding throw wins over a closing throw');
assert.compareArray(log, ['next', 'default', 'close']);

function constAssignment() {
  const fixed = 1;
  var log = [];
  assert.throws(TypeError, function () {
    for (fixed of single(2, log)) {}
  });
  assert.compareArray(log, ['next', 'close']);
  assert.throws(TypeError, function () { for (fixed in { x: 0 }) {} });
}
constAssignment();

function tdzAssignment() {
  var log = [];
  assert.throws(ReferenceError, function () {
    for (uninitialized of single(2, log)) {}
  });
  assert.compareArray(log, ['next', 'close']);
  let uninitialized;
}
tdzAssignment();

// The Function constructor gives this web-compatible call target a sloppy
// context even when the surrounding test is running its strict scenario.
var invalidCall = Function('call', 'iterable', 'for (call() of iterable) {}');
var calls = 0;
log = [];
assert.throws(ReferenceError, function () {
  invalidCall(function () { calls++; return {}; }, single(1, log));
});
assert.sameValue(calls, 1);
assert.compareArray(log, ['next', 'close']);
invalidCall(function () { calls++; }, []);
assert.sameValue(calls, 1, 'an empty iterator never evaluates the target');

log = [];
for (using item of [resource(log, 'a'), resource(log, 'b')]) {
  log.push('body ' + item.name);
}
assert.compareArray(log, [
  'get a', 'body a', 'dispose a', 'get b', 'body b', 'dispose b'
]);
log = [];
caught = undefined;
var broken = { get [Symbol.dispose]() { log.push('get'); throw original; } };
try {
  for (using item of single(broken, log, closing)) log.push('unreachable');
} catch (error) { caught = error; }
assert.sameValue(caught, original, 'registration fails before initialization and body');
assert.compareArray(log, ['next', 'get', 'close']);

// A direct eval keeps these generators on the state-machine path in the
// default mode as well as under --no-bytecode.
function* generatorHeads() {
  eval('');
  var readers = [];
  for (let own of [7, 8]) {
    readers.push(function () { return own; });
    yield own;
  }
  assert.compareArray(valuesOf(readers), [7, 8], 'generator lexical captures');
  var target;
  for ({ value: target } of [{ value: 9 }]) yield target;
  for (var { value: shared } of [{ value: 10 }]) yield shared;
  for (const key in { ab: 0 }) yield key;
  var log = [];
  for (using item of [resource(log, 'generator')]) yield item.name;
  assert.compareArray(log, ['get generator', 'dispose generator']);
}

var generator = generatorHeads();
var yielded = [];
var step;
while (!(step = generator.next()).done) yielded.push(step.value);
assert.compareArray(yielded, [7, 8, 9, 10, 'ab', 'generator']);

function* generatorBindingFailure() {
  eval('');
  for (let { value = failBinding() } of single({}, log, closing)) yield value;
}
log = [];
caught = undefined;
try { generatorBindingFailure().next(); } catch (error) { caught = error; }
assert.sameValue(caught, original, 'generator binding failure retains the throw');
assert.compareArray(log, ['next', 'default', 'close']);

async function asyncHeads() {
  eval('');
  var readers = [];
  for (let own of [11, 12]) {
    await null;
    readers.push(function () { return own; });
  }
  assert.compareArray(valuesOf(readers), [11, 12], 'async lexical captures');
  var assigned;
  for ({ value: assigned } of [{ value: 13 }]) await null;
  assert.sameValue(assigned, 13);
  for (var { value: shared } of [{ value: 14 }]) await null;
  assert.sameValue(shared, 14);
  for (assigned in { cd: 0 }) await null;
  assert.sameValue(assigned, 'cd');
  var log = [];
  for (using item of [resource(log, 'async')]) {
    await null;
    log.push('body ' + item.name);
  }
  assert.compareArray(log, ['get async', 'body async', 'dispose async']);
  log = [];
  var asynchronousResource = {
    get [Symbol.asyncDispose]() {
      log.push('get asyncDispose');
      return function () {
        log.push('asyncDispose');
        return Promise.resolve();
      };
    }
  };
  for (await using item of [asynchronousResource]) log.push('await-using body');
  assert.compareArray(log, ['get asyncDispose', 'await-using body', 'asyncDispose']);
  var awaited = [];
  for await (let { value } of [{ value: 15 }, { value: 16 }]) {
    awaited.push(value);
  }
  assert.compareArray(awaited, [15, 16], 'for-await destructuring bindings');
  for await ({ value: assigned } of [{ value: 17 }]) {}
  assert.sameValue(assigned, 17, 'for-await assignment');
}

async function asyncBindingFailure(isAwait) {
  eval('');
  if (isAwait) {
    for await (let { value = failBinding() } of single({}, log, closing)) {
      await null;
    }
  } else {
    for (let { value = failBinding() } of single({}, log, closing)) await null;
  }
}

async function* asyncGeneratorHeads() {
  eval('');
  var readers = [];
  for (const own of [18, 19]) {
    readers.push(function () { return own; });
    yield own;
  }
  assert.compareArray(valuesOf(readers), [18, 19], 'async generator lexical captures');
  var assigned;
  for ({ value: assigned } of [{ value: 20 }]) yield assigned;
  for (var { value: shared } of [{ value: 21 }]) yield shared;
  var log = [];
  for (using item of [resource(log, 'async generator')]) yield item.name;
  assert.compareArray(log, ['get async generator', 'dispose async generator']);
  for await (const { value } of [{ value: 22 }]) yield value;
}

async function* asyncGeneratorBindingFailure() {
  eval('');
  for (let { value = failBinding() } of single({}, log, closing)) yield value;
}

function rejectsOriginal(promise, message) {
  return promise.then(function () {
    throw new Test262Error(message + ' must reject');
  }, function (error) {
    assert.sameValue(error, original, message);
    assert.compareArray(log, ['next', 'default', 'close']);
  });
}

asyncHeads()
  .then(function () {
    log = [];
    return rejectsOriginal(asyncBindingFailure(false), 'async binding throw');
  })
  .then(function () {
    log = [];
    return rejectsOriginal(asyncBindingFailure(true), 'for-await binding throw');
  })
  .then(async function () {
    var yielded = [];
    for await (const value of asyncGeneratorHeads()) yielded.push(value);
    assert.compareArray(yielded, [18, 19, 20, 21, 'async generator', 22]);
  })
  .then(function () {
    log = [];
    return rejectsOriginal(asyncGeneratorBindingFailure().next(), 'async generator binding throw');
  })
  .then($DONE, $DONE);
