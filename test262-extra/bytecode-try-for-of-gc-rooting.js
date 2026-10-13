/*---
description: >
  Values the VM keeps live across statement boundaries survive collections:
  the iterator of a running for-of, the enumerator of a for-in, the thrown
  value waiting in a finalizer's completion record, and a return value
  waiting for finalizers and iterator closes.
info: |
  jsse issue #960: these are operand-stack slots rooted in `gc_bytecode_roots`;
  a handler cuts the stack back and must keep exactly the slots below it
  rooted.
esid: sec-try-statement-runtime-semantics-evaluation
flags: [noStrict]
---*/

function make(tag) { return { tag: tag, list: [tag, tag + 1] }; }
function churn() {
  var junk = [];
  for (var n = 0; n < 100; n++) junk.push({ n: n, pad: [n, n + 1] });
  return junk.length;
}
function collect() { $262.gc(); churn(); $262.gc(); churn(); }

function iteratorSurvivesTheBody() {
  var seen = [];
  var closed = 0;
  var it = {
    [Symbol.iterator]() {
      var i = 0;
      return {
        next() { return { value: make(i), done: ++i > 4 }; },
        return() { closed++; return {}; },
      };
    },
  };
  for (var v of it) {
    collect();
    seen.push(v.tag);
    if (v.tag === 2) break;
  }
  assert.sameValue(seen.join(), "0,1,2");
  assert.sameValue(closed, 1);
}
iteratorSurvivesTheBody();

function enumeratorSurvivesTheBody() {
  var o = {};
  for (var i = 0; i < 20; i++) o["k" + i] = make(i);
  var count = 0;
  for (var k in o) {
    collect();
    count += o[k].tag === count ? 1 : 0;
  }
  assert.sameValue(count, 20);
}
enumeratorSurvivesTheBody();

function thrownValueSurvivesTheFinalizer() {
  var caught;
  try {
    try { throw make(7); } finally { collect(); collect(); }
  } catch (e) { caught = e; }
  assert.sameValue(caught.tag, 7);
  assert.sameValue(caught.list[1], 8);
}
thrownValueSurvivesTheFinalizer();

function returnValueSurvivesFinalizersAndClose() {
  var closed = 0;
  var it = {
    [Symbol.iterator]() {
      return { next() { return { value: 1, done: false }; }, return() { collect(); closed++; return {}; } };
    },
  };
  function f() {
    for (var x of it) {
      for (var k in { a: 1 }) {
        try { return make(9); } finally { collect(); }
      }
    }
  }
  var r = f();
  assert.sameValue(r.tag, 9);
  assert.sameValue(r.list[0], 9);
  assert.sameValue(closed, 1);
}
returnValueSurvivesFinalizersAndClose();

function handlerKeepsOlderOperandsRooted() {
  function boom() { collect(); throw make(1); }
  var keep = make(2);
  var r;
  try {
    r = [make(3), keep, boom(), make(4)];
  } catch (e) {
    collect();
    r = [e.tag, keep.tag];
  }
  assert.sameValue(r.join(), "1,2");
}
handlerKeepsOlderOperandsRooted();

function nestedLoopsAndTries() {
  var total = 0;
  for (var a of [make(1), make(2)]) {
    try {
      for (var b of [make(10), make(20)]) {
        try {
          if (b.tag === 20) throw make(a.tag);
          total += b.tag;
        } finally {
          collect();
        }
      }
    } catch (e) {
      total += e.tag * 100;
    }
  }
  assert.sameValue(total, 10 + 100 + 10 + 200);
}
nestedLoopsAndTries();
