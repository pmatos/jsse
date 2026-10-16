/*---
description: >
  A declaration-free block in an async function or async generator that holds
  an await or yield behaves as if it had its own scope: a var declared in it
  is function-scoped, a function declaration in it follows Annex B, and a
  closure created in it inside a for (let ...) loop captures the per-iteration
  binding.
esid: sec-block-runtime-semantics-evaluation
flags: [async, noStrict]
includes: [asyncHelpers.js, compareArray.js]
features: [async-functions, async-iteration]
---*/

asyncTest(async function () {
  async function varInBlock() {
    { await 1; var z = 2; }
    return z;
  }
  assert.sameValue(await varInBlock(), 2, "var declared in a block after await");

  async function loopLetClosures() {
    const fs = [];
    for (let i = 0; i < 3; i++) { { await 0; fs.push(() => i); } }
    return fs.map(f => f());
  }
  assert.compareArray(await loopLetClosures(), [0, 1, 2], "per-iteration let captured");

  async function* gen() {
    for (let i = 0; i < 2; i++) { { yield i; await 0; } }
    return "end";
  }
  const out = [];
  for await (const v of gen()) out.push(v);
  assert.compareArray(out, [0, 1], "async generator yields through a block");

  async function blockFunction() {
    { await 1; function inner() {} }
    return typeof inner;
  }
  assert.sameValue(await blockFunction(), "function", "Annex B block function leaks to function scope");
});
