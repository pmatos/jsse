/*---
description: >
  A function declaration in a block, switch case block, or try/catch/finally
  clause is initialized when the block is entered, so it can be called before
  its declaration even when a `yield` or `await` separates the two.
esid: sec-blockdeclarationinstantiation
info: |
  BlockDeclarationInstantiation ( code, env )

  4. For each element d of declarations, do
    ...
    c. If d is either a FunctionDeclaration, a GeneratorDeclaration, an
       AsyncFunctionDeclaration, or an AsyncGeneratorDeclaration, then
      i. Let fn be the sole element of the BoundNames of d.
      ii. Let fo be InstantiateFunctionObject of d with arguments env and privateEnv.
      iii. Perform ! env.InitializeBinding(fn, fo).
flags: [async]
includes: [compareArray.js]
features: [generators, async-functions, async-iteration]
---*/

function* inBlock() {
  yield* [];
  { yield inner(); function inner() { return "block"; } }
}

function* inBlockAfterLet() {
  yield* [];
  { let a = "a"; yield inner() + a; function inner() { return "block+"; } }
}

function* inSwitch() {
  yield* [];
  switch (1) { case 1: yield inner(); case 2: function inner() { return "switch"; } }
}

function* inSuspendingSwitch() {
  yield* [];
  switch (yield "disc") { case yield "case": yield inner(); break; default: function inner() { return "switch2"; } }
}

function* inTry() {
  yield* [];
  try { yield inner(); function inner() { return "try"; } } finally { }
}

function* inCatch() {
  yield* [];
  try { throw 1; } catch (e) { yield inner(); function inner() { return "catch"; } }
}

function* inFinally() {
  yield* [];
  try { } finally { yield inner(); function inner() { return "finally"; } }
}

function* inLoopBody() {
  yield* [];
  for (var i = 0; i < 2; i++) { yield inner() + i; function inner() { return "loop"; } }
}

function* inArrowStrictBlock() {
  "use strict";
  yield* [];
  { yield inner(); function inner() { return "strict"; } }
}

async function asyncInBlock() {
  { await 0; return inner(); function inner() { return "async block"; } }
}

async function* asyncGenInBlock() {
  { yield inner(); function inner() { return "async gen block"; } }
}

function drain(it, sends) {
  var out = [];
  var r = it.next();
  var k = 0;
  while (!r.done) {
    out.push(r.value);
    r = it.next(sends ? sends[k++] : undefined);
  }
  return out;
}

assert.compareArray(drain(inBlock()), ["block"], "block");
assert.compareArray(drain(inBlockAfterLet()), ["block+a"], "block after let");
assert.compareArray(drain(inSwitch()), ["switch"], "switch case");
assert.compareArray(drain(inSuspendingSwitch(), [1, 1]), ["disc", "case", "switch2"], "switch with suspending tests");
assert.compareArray(drain(inTry()), ["try"], "try block");
assert.compareArray(drain(inCatch()), ["catch"], "catch block");
assert.compareArray(drain(inFinally()), ["finally"], "finally block");
assert.compareArray(drain(inLoopBody()), ["loop0", "loop1"], "loop body");
assert.compareArray(drain(inArrowStrictBlock()), ["strict"], "strict block");

asyncInBlock().then(function(v) {
  assert.sameValue(v, "async block", "async function block");
  return asyncGenInBlock().next();
}).then(function(r) {
  assert.sameValue(r.value, "async gen block", "async generator block");
}).then($DONE, $DONE);
