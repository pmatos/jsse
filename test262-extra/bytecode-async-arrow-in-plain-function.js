/*---
description: >
  A plain function whose body contains an async arrow function that uses
  `await` or `for await` behaves exactly as ordinary function semantics
  require: the arrow owns the `await`, captures the enclosing `this`,
  `arguments`, per-iteration bindings and block scopes, and the enclosing
  function itself runs to completion synchronously.
info: |
  jsse issue #907: an `await` inside a nested async arrow no longer keeps the
  enclosing plain function's body on the tree-walker.
esid: sec-async-arrow-function-definitions-runtime-semantics-evaluation
flags: [async, noStrict]
includes: [asyncHelpers.js, compareArray.js]
features: [async-functions, async-iteration]
---*/

var log = [];

function reduceLike(data, fn, initial) {
  var acc = initial;
  if (data instanceof Promise) {
    return (async () => {
      let stream = await data;
      let i = 0;
      for await (let datum of stream) acc = fn(acc, datum, i++);
      return acc;
    })();
  }
  let i = 0;
  for (let datum of data) acc = fn(acc, datum, i++);
  return acc;
}

function capturing() {
  var self = this;
  var args = arguments;
  var fns = [];
  for (let i = 0; i < 3; i++) {
    fns.push(async () => {
      await null;
      return [this === self, arguments === args, i, args.length];
    });
  }
  {
    let blockLocal = "block";
    fns.push(async () => {
      await 0;
      return blockLocal;
    });
  }
  log.push("sync-end");
  return fns;
}

function tryCatchInside(flag) {
  try {
    if (flag) throw new Error("boom");
    return (async () => await "ok")();
  } catch (e) {
    return (async () => { await null; return e.message; })();
  } finally {
    log.push("finally");
  }
}

var arrowOwner = (tag) => {
  var inner = async () => { await null; return tag; };
  return inner;
};

function classPositions(base) {
  var key = "k";
  class C extends base { [key + "1"]() { return 1; } }
  var viaExtends = (async () => { await null; return new C().k1(); })();
  return viaExtends;
}

function awaitAsIdentifier() {
  var await = 7;
  var yield = 8;
  return await + yield;
}

function generatorInside() {
  function* g() { yield 1; yield 2; }
  return [...g()];
}

assert.sameValue(reduceLike([1, 2, 3], (a, x) => a + x, 0), 6, "sync branch of a function that also owns an async arrow");
assert.sameValue(awaitAsIdentifier(), 15, "await and yield are ordinary identifiers in sloppy plain functions");
assert.compareArray(generatorInside(), [1, 2]);

asyncTest(async function () {
  assert.sameValue(
    await reduceLike(Promise.resolve([1, 2, 3]), (a, x) => a + x, 10),
    16,
    "for await inside the nested async arrow"
  );

  var fns = capturing.call({ marker: 1 }, "a", "b");
  assert.compareArray(log, ["sync-end"], "the plain function finished before any arrow body resumed");
  var results = await Promise.all(fns.map(f => f()));
  assert.compareArray(results[0], [true, true, 0, 2]);
  assert.compareArray(results[1], [true, true, 1, 2]);
  assert.compareArray(results[2], [true, true, 2, 2]);
  assert.sameValue(results[3], "block");

  assert.sameValue(await arrowOwner("from-arrow")(), "from-arrow", "async arrow inside a plain arrow");
  assert.sameValue(await classPositions(class {}), 1, "async arrow beside a class with computed key and extends");

  assert.sameValue(await tryCatchInside(false), "ok");
  assert.sameValue(await tryCatchInside(true), "boom");
  assert.compareArray(log, ["sync-end", "finally", "finally"]);
});
