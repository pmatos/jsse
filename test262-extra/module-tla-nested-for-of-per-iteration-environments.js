/*---
description: >
  Top-level await in a nested for-of preserves the outer loop's lexical
  iteration environment instead of exposing uninitialized module bindings.
esid: sec-runtime-semantics-forin-div-ofbodyevaluation-lhs-stmt-iterator-lhskind-labelset
info: |
  ForIn/OfBodyEvaluation evaluates each lexical loop body in a fresh
  declarative environment whose outer environment is the environment active
  when the loop began. AsyncBlockStart and Await resume module evaluation with
  that complete execution-context state.
flags: [module]
includes: [compareArray.js]
features: [async-functions, top-level-await]
---*/

const forms = { a: async function () { return 1; }, b: async function () { return 2; } };
const observed = [];

for (const type of ['text', 'bytes']) {
  const attrs = { with: { type: type } };
  const messages = [];

  for (const name of Object.keys(forms)) {
    const result = await forms[name](attrs);
    messages.push(result);
  }

  observed.push(type + ':' + messages.join(',') + ':' + attrs.with.type);
}

assert.compareArray(
  observed,
  ['text:1,2:text', 'bytes:1,2:bytes'],
  'top-level await retains outer for-of bindings and body declarations'
);
