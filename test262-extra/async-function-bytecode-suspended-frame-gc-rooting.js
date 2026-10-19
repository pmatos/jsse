// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-async-function-definitions-runtime-semantics-evaluation
description: >
  The operand stack, block environments, for-of iterators and result promise
  of an async function body that is suspended at an await are kept alive by
  the suspended activation alone, across minor and major collections, and the
  function resumes with every value intact.
flags: [async]
includes: [compareArray.js]
features: [async-functions, host-gc-required]
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

async function operands() {
  var result = [{ tag: "first" }, [1, 2, 3], new Map([["k", "v"]]), await later("sent"), { tag: "last" }];
  return result;
}

async function callOperands() {
  var obj = { name: "recv", m(a, b, c) { return this.name + a.x + b.y + c; } };
  return obj.m({ x: 1 }, { y: 2 }, await later(3));
}

async function assignTarget() {
  var holder = {};
  (function () { return holder; })().slot = await later("slot-value");
  return holder.slot;
}

async function blockScopes() {
  var seen = [];
  for (const outer of [{ id: "o1" }, { id: "o2" }]) {
    const inner = { id: outer.id + "-inner", list: [outer] };
    for (const item of [{ n: 1 }, { n: 2 }]) {
      const doubled = { n: item.n * 2 };
      await later(0);
      seen.push(outer.id + inner.list.length + item.n + doubled.n);
    }
  }
  return seen.join(",");
}

async function iteratorHeldAcrossAwait() {
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
  var total = 0;
  for (const entry of iterable) {
    await later(0);
    total += entry.v;
    if (total >= 1) break;
  }
  return total + ":" + closed.join();
}

async function tryAndFinallyRecords() {
  try {
    return { kept: await later({ tag: "ret" }) };
  } finally {
    var pending = { tag: "fin" };
    await later(0);
    log.push("finally:" + pending.tag);
  }
}

var droppedSideEffect = [];
async function resultPromiseOnlyHeldByActivation() {
  var held = { tag: "held" };
  await later(0);
  droppedSideEffect.push(held.tag);
  return held;
}

(async function () {
  var r = await operands();
  assert.sameValue(r[0].tag, "first");
  assert.compareArray(r[1], [1, 2, 3]);
  assert.sameValue(r[2].get("k"), "v");
  assert.sameValue(r[3], "sent");
  assert.sameValue(r[4].tag, "last");

  assert.sameValue(await callOperands(), "recv123");
  assert.sameValue(await assignTarget(), "slot-value");
  assert.sameValue(await blockScopes(), "o1112,o1124,o2112,o2124");
  assert.sameValue(await iteratorHeldAcrossAwait(), "1:it");
  var t = await tryAndFinallyRecords();
  assert.sameValue(t.kept.tag, "ret");
  assert.compareArray(log, ["finally:fin"]);

  resultPromiseOnlyHeldByActivation();
  collect();
  await later(0);
  await later(0);
  assert.compareArray(droppedSideEffect, ["held"]);
})().then($DONE, $DONE);
