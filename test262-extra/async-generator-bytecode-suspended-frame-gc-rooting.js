// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-asyncgenerator-prototype-next
description: >
  The operand stack, block environments, for-of iterators and variables of an
  async generator body suspended at a yield or at an await are kept alive by
  the generator alone, across minor and major collections, and a generator
  that only its pending request and await reaction reference still finishes
  and settles its requests.
flags: [async]
includes: [compareArray.js]
features: [async-iteration, host-gc-required]
---*/

function churn() {
  var junk = [];
  for (var i = 0; i < 200; i++) junk.push({ i: i, s: "x" + i, a: [i] });
  return junk.length;
}

function collect() {
  churn();
  $262.gc();
  churn();
}

function later(value) {
  return new Promise(function (resolve) {
    Promise.resolve().then(function () {
      collect();
      resolve(value);
    });
  });
}

var log = [];

async function* operands() {
  var result = [{ tag: "first" }, [1, 2, 3], new Map([["k", "v"]]), yield "pause", { tag: "last" }];
  return result;
}

async function* awaitedOperands() {
  var result = [{ tag: "a" }, await later("w"), { tag: "b" }];
  yield result;
}

async function* scopes() {
  for (const outer of [{ id: "o1" }, { id: "o2" }]) {
    const inner = { id: outer.id + "-inner", list: [outer] };
    for (const item of [{ n: 1 }, { n: 2 }]) {
      const doubled = { n: item.n * 2 };
      yield await later(outer.id + inner.list.length + item.n + doubled.n);
    }
  }
}

async function* iteratorHeld() {
  var closed = [];
  var iterable = {
    [Symbol.iterator]() {
      var state = { n: 0, payload: { tag: "it" } };
      return {
        next() { return { done: state.n >= 3, value: { v: state.n++ } }; },
        return() { closed.push(state.payload.tag); return {}; },
      };
    },
  };
  for (const entry of iterable) {
    yield entry.v;
  }
  yield closed.join();
}

async function* finallyRecords() {
  try {
    yield { tag: "y" };
  } finally {
    var pending = { tag: "fin" };
    await later(0);
    log.push("finally:" + pending.tag);
  }
}

(async function () {
  var g = operands();
  await g.next();
  collect();
  var r = (await g.next("sent")).value;
  assert.sameValue(r[0].tag, "first");
  assert.compareArray(r[1], [1, 2, 3]);
  assert.sameValue(r[2].get("k"), "v");
  assert.sameValue(r[3], "sent");
  assert.sameValue(r[4].tag, "last");

  var w = (await awaitedOperands().next()).value;
  assert.sameValue(w[0].tag, "a");
  assert.sameValue(w[1], "w");
  assert.sameValue(w[2].tag, "b");

  var seen = [];
  for await (const v of scopes()) {
    collect();
    seen.push(v);
  }
  assert.compareArray(seen, ["o1112", "o1124", "o2112", "o2124"]);

  var it = iteratorHeld();
  var values = [];
  for await (const v of it) {
    collect();
    values.push(v);
  }
  assert.compareArray(values, [0, 1, 2, ""]);

  var fr = finallyRecords();
  await fr.next();
  collect();
  var done = await fr.return("R");
  assert.sameValue(done.value, "R");
  assert.compareArray(log, ["finally:fin"]);

  // Only the pending request and the await reaction reference the generator.
  var order = [];
  (function () {
    var temp = (async function* () {
      await later(0);
      order.push("body");
      yield "v";
    })();
    temp.next().then(function (r) { order.push("settled:" + r.value); });
  })();
  collect();
  await later(0);
  await later(0);
  await later(0);
  assert.compareArray(order, ["body", "settled:v"]);
})().then($DONE, $DONE);
