/*---
description: >
  A value assigned in statement context to a global accessor stays reachable
  while the setter runs and forces a collection: the discarding store must pop
  the value only after the store has completed.
info: |
  sec-assignment-operators-runtime-semantics-evaluation step 1.e.i (PutValue
  runs the setter with the freshly produced value).
esid: sec-assignment-operators-runtime-semantics-evaluation
flags: [noStrict]
---*/

var seen = [];

Object.defineProperty(globalThis, "sink", {
  configurable: true,
  set: function (v) {
    $262.gc();
    $262.gc();
    var churn = [];
    for (var i = 0; i < 64; i++) { churn.push({ filler: i }); }
    seen.push(v.tag + ":" + v.payload.length);
  },
});

function fresh(tag) {
  return { tag: tag, payload: [1, 2, 3, 4] };
}

function assignStatement() { sink = fresh("statement"); }
function assignForInit() { for (sink = fresh("init"); false;) {} }
function assignComma() { sink = fresh("comma-a"), sink = fresh("comma-b"); }
function assignVar() { var local = fresh("var"); sink = local; }

assignStatement();
assignForInit();
assignComma();
assignVar();

assert.sameValue(
  seen.join(","),
  "statement:4,init:4,comma-a:4,comma-b:4,var:4"
);
