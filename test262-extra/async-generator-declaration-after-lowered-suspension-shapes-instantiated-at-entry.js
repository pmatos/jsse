/*---
description: >
  Function declarations after try/catch/finally, switch, labelled-block,
  for-await, yield* and arrow/method suspension shapes are instantiated at
  function entry, and a declaration closing over a later let/const still
  observes its TDZ.
esid: sec-functiondeclarationinstantiation
info: |
  FunctionDeclarationInstantiation initializes every function declaration of
  the body before it runs, whichever lowered state the declaration follows.
  Lexical declarations are instantiated uninitialized (TDZ) in the same step.

  Regression test for issue #1032.
flags: [async]
---*/

async function afterTry() {
  try { await 0; throw 1; } catch (e) { await 0; return s(); }
  function s() { return "catch"; }
}

async function afterSwitch(k) {
  switch (k) {
    case 1: await 0; return s();
    default: return s();
  }
  function s() { return "switch"; }
}

async function afterLabelledBlock() {
  l: { await 0; break l; }
  return s();
  function s() { return "labelled"; }
}

async function afterForAwait() {
  for await (var x of [1]) { await 0; }
  return s();
  function s() { return "for-await"; }
}

async function afterFinally() {
  try { await 0; } finally { await 0; }
  return s();
  function s() { return "finally"; }
}

var arrow = async () => {
  await 0;
  return s();
  function s() { return "arrow"; }
};

class C {
  async method() {
    await 0;
    return s();
    function s() { return "method"; }
  }
  static async *gen() {
    yield s();
    function s() { return "static-gen"; }
  }
}

function* finallyYield() {
  try { yield 1; } finally { yield s(); }
  function s() { return "gen-finally"; }
}

function* afterDelegate() {
  yield* [1];
  yield s();
  function s() { return "gen-delegate"; }
}

function* tdz() {
  yield f();
  let x = 1;
  function f() { return x; }
}

var fin = finallyYield();
fin.next();
assert.sameValue(fin.return(5).value, "gen-finally", "after try/finally");

var del = afterDelegate();
del.next();
assert.sameValue(del.next().value, "gen-delegate", "after yield*");

assert.throws(ReferenceError, function () {
  tdz().next();
}, "declaration reads a later let in its TDZ");

Promise.all([
  afterTry(),
  afterSwitch(1),
  afterSwitch(2),
  afterLabelledBlock(),
  afterForAwait(),
  afterFinally(),
  arrow(),
  new C().method(),
  C.gen().next(),
]).then(function (r) {
  assert.sameValue(r[0], "catch");
  assert.sameValue(r[1], "switch");
  assert.sameValue(r[2], "switch");
  assert.sameValue(r[3], "labelled");
  assert.sameValue(r[4], "for-await");
  assert.sameValue(r[5], "finally");
  assert.sameValue(r[6], "arrow");
  assert.sameValue(r[7], "method");
  assert.sameValue(r[8].value, "static-gen");
}).then($DONE, $DONE);
